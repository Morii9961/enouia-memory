//! MV-3.1/3.2 acceptance: candidates never reach the canonical view (M01),
//! agents cannot approve and imported "remember" text writes nothing (M02),
//! edits and stale batches are exact and all-or-nothing (M03), evidence
//! roles come from the sources (M04), contradictions become visible
//! conflict groups (M07), plus merge, duplicates, withdrawal, and undoing a
//! wrong acceptance.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::candidate::{
    CandidateRecord, CandidateStatus, MergeTarget, ProposalKind, ProposedType, ReviewAction,
    ReviewRecord,
};
use enouia_memory_contract::common::{EvidenceClass, Sensitivity, TrustedSurface};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::CandidateId;
use enouia_memory_contract::memory::MemoryStatus;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_govern::propose::{CandidateEdit, withdraw};
use enouia_memory_govern::{
    Canonical, Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed,
    canonical_memories, confirm, edit_candidate, pending_candidates, plan, propose,
};
use enouia_memory_vault::{Fault, VaultError};
use serde_json::Map;
use support::{Env, agent, fact, owner, rev};

const CLI: TrustedSurface = TrustedSurface::TrustedLocalCli;

#[cfg(windows)]
#[test]
fn native_host_writer_preserves_pending_proposals_and_exact_owner_review() {
    use enouia_memory_contract::ids::DeviceId;
    use enouia_memory_vault::host_transaction::HostCoordinator;
    use enouia_memory_vault::{OsIdSource, RootPolicy, Vault, VaultOptions, verify_data_root};
    use std::sync::Arc;

    let env = Env::new("native-host-governance");
    let source = env.statement("合成：项目使用本地文件存储。");
    let root = verify_data_root(&env.root.join("vault"), &RootPolicy::default()).unwrap();
    enouia_memory_vault::platform::restrict_to_owner(root.path()).unwrap();
    let identity = DeviceId::parse("dev_00000001-0000-4000-8000-000000000001").unwrap();
    drop(HostCoordinator::bootstrap(&env.vault, &root, identity.clone()).unwrap());
    let writer = Vault::open(
        &root,
        None,
        env.clock.clone(),
        Arc::new(OsIdSource),
        VaultOptions::default(),
    )
    .unwrap()
    .into_host_writer(&root, identity)
    .unwrap();
    let before = writer.host_writer_state().unwrap();
    env.tick();
    let proposal = fact(&source, "host.storage", "项目使用本地文件存储。");
    let Proposed::Stored(written) = propose(
        &writer,
        &proposal,
        &Origin::agent(agent()),
        b"native-host-proposal",
    )
    .unwrap() else {
        panic!("new native proposal must be pending");
    };
    let pending_pin = writer.pin_current().unwrap();
    assert_eq!(pending_candidates(&writer, &pending_pin).unwrap().len(), 1);
    assert!(
        canonical_memories(&writer, &pending_pin, Canonical::Active)
            .unwrap()
            .is_empty()
    );
    assert!(plan(&writer, &[accept(&written.id, 1)], &agent(), CLI).is_err());
    let shown = plan(&writer, &[accept(&written.id, 1)], &owner(), CLI).unwrap();
    env.tick();
    confirm(&writer, &shown, &yes()).unwrap();
    let accepted_pin = writer.pin_current().unwrap();
    assert!(
        pending_candidates(&writer, &accepted_pin)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        canonical_memories(&writer, &accepted_pin, Canonical::Active)
            .unwrap()
            .len(),
        1
    );
    let after = writer.host_writer_state().unwrap();
    assert_eq!(after.revision.get(), before.revision.get() + 2);
    assert_eq!(after.checkpoint, writer.verified_data_checkpoint().unwrap());
    assert!(
        propose(
            &env.vault,
            &proposal,
            &Origin::agent(agent()),
            b"legacy-writer"
        )
        .is_err()
    );
}

fn yes() -> OwnerConfirmation {
    OwnerConfirmation {
        owner: owner(),
        surface: CLI,
    }
}

fn accept(id: &CandidateId, revision: u64) -> Decision {
    Decision::Accept {
        candidate_id: id.clone(),
        revision: rev(revision),
    }
}

fn decide(env: &Env, decisions: &[Decision]) -> CommitOutcome {
    env.tick();
    let shown = plan(&env.vault, decisions, &owner(), CLI).unwrap();
    confirm(&env.vault, &shown, &yes()).unwrap()
}

fn rules(error: &VaultError) -> Vec<&'static str> {
    match &error.fault {
        Fault::Contract(rules) => rules.clone(),
        _ => Vec::new(),
    }
}

fn candidate(env: &Env, id: &CandidateId) -> CandidateRecord {
    let pin = env.pin();
    let entry = env
        .vault
        .record_entry(&pin, RecordKind::Candidate, id.as_str())
        .unwrap()
        .unwrap();
    let bytes = env
        .vault
        .read_record(
            &pin,
            &RecordRef::new(RecordKind::Candidate, id.as_str(), entry.revision),
        )
        .unwrap();
    enouia_memory_contract::parse_record(&bytes).unwrap()
}

fn review_of(env: &Env, outcome: &CommitOutcome) -> ReviewRecord {
    let receipt = match outcome {
        CommitOutcome::Committed { receipt, .. } | CommitOutcome::Replayed { receipt, .. } => {
            receipt
        }
    };
    let reference = receipt
        .records
        .iter()
        .find(|r| r.record_kind == RecordKind::Review)
        .unwrap();
    let bytes = env.vault.read_record(&env.pin(), reference).unwrap();
    enouia_memory_contract::parse_record(&bytes).unwrap()
}

#[test]
fn bound_publication_race_returns_the_winning_id_or_payload_conflict() {
    use enouia_memory_contract::ports::IdSource;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;
    struct PausedIds {
        first: AtomicBool,
        ready: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl IdSource for PausedIds {
        fn random_16(&self) -> [u8; 16] {
            if !self.first.swap(true, Ordering::SeqCst) {
                self.ready.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
            }
            enouia_memory_vault::OsIdSource.random_16()
        }
    }
    for changed in [false, true] {
        let env = Env::new("bound-publication-race");
        let source = env.statement("合成的绑定发布请求。");
        let proposal = fact(&source, "bound_publication", "合成的绑定发布请求。");
        let mut losing = proposal.clone();
        if changed {
            losing.content = "另一条合成发布请求。".into();
        }
        let origin = Origin::owner(owner());
        let payload = sha256(b"synthetic-owner-choice");
        let losing_payload = if changed {
            sha256(b"different-owner-choice")
        } else {
            payload.clone()
        };
        let base = env.pin();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let root = enouia_memory_vault::verify_data_root(
            env.vault.managed_root().root(),
            &enouia_memory_vault::RootPolicy::default(),
        )
        .unwrap();
        let paused = enouia_memory_vault::Vault::open(
            &root,
            None,
            env.clock.clone(),
            Arc::new(PausedIds {
                first: AtomicBool::new(false),
                ready: ready_tx,
                release: Mutex::new(release_rx),
            }),
            enouia_memory_vault::VaultOptions::default(),
        )
        .unwrap();
        std::thread::scope(|scope| {
            let second = scope.spawn(|| {
                enouia_memory_govern::propose_bound_at_head(
                    &paused,
                    &losing,
                    &origin,
                    b"shared-operation",
                    &losing_payload,
                    &base.commit_id,
                )
            });
            ready_rx.recv_timeout(Duration::from_secs(30)).unwrap();
            let first = enouia_memory_govern::propose_bound_at_head(
                &env.vault,
                &proposal,
                &origin,
                b"shared-operation",
                &payload,
                &base.commit_id,
            );
            release_tx.send(()).unwrap();
            let second = second.join().unwrap();
            let Proposed::Stored(written) = first.unwrap() else {
                panic!("stored")
            };
            if changed {
                assert_eq!(
                    second.unwrap_err().code(),
                    MemoryErrorCode::IdempotencyConflict
                );
            } else {
                let Proposed::Stored(replayed) = second.unwrap() else {
                    panic!("replayed")
                };
                assert_eq!(written.id, replayed.id);
                assert!(matches!(replayed.outcome, CommitOutcome::Replayed { .. }));
            }
            let pin = env.pin();
            assert_eq!(pin.sequence, base.sequence + 1);
            assert_eq!(
                pending_candidates(&env.vault, &pin).unwrap()[0].candidate_id,
                written.id
            );
            assert!(
                canonical_memories(&env.vault, &pin, Canonical::Active)
                    .unwrap()
                    .is_empty()
            );
            env.assert_history_valid();
        });
    }
}

#[test]
fn m01_only_accepted_candidates_reach_the_canonical_view() {
    let env = Env::new("m01");
    let said = env.statement("（合成）我用 VS Code 写代码。");
    let a = env.propose_fact(
        &Origin::owner(owner()),
        &said,
        "tool.editor",
        "用 VS Code 写代码。",
    );
    let b = env.propose_fact(
        &Origin::agent(agent()),
        &said,
        "tool.shell",
        "常用 PowerShell。",
    );
    let c = env.propose_fact(&Origin::agent(agent()), &said, "tool.os", "用 Windows 11。");
    // Pending, rejected, and withdrawn candidates stay out.
    assert!(
        canonical_memories(&env.vault, &env.pin(), Canonical::Active)
            .unwrap()
            .is_empty()
    );
    decide(
        &env,
        &[Decision::Reject {
            candidate_id: b.clone(),
            revision: rev(1),
            reason_code: Some("not_mine".into()),
        }],
    );
    env.tick();
    withdraw(&env.vault, &c, rev(1), &agent(), &env.key()).unwrap();
    assert!(
        canonical_memories(&env.vault, &env.pin(), Canonical::AllStatuses)
            .unwrap()
            .is_empty()
    );
    assert_eq!(pending_candidates(&env.vault, &env.pin()).unwrap().len(), 1);
    decide(&env, &[accept(&a, 1)]);
    let view = canonical_memories(&env.vault, &env.pin(), Canonical::Active).unwrap();
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].content, "用 VS Code 写代码。");
    assert_eq!(candidate(&env, &b).status, CandidateStatus::Rejected);
    assert_eq!(candidate(&env, &c).status, CandidateStatus::Withdrawn);
    assert!(
        pending_candidates(&env.vault, &env.pin())
            .unwrap()
            .is_empty()
    );
    env.assert_history_valid();
}

#[test]
fn m02_agents_cannot_approve_and_imported_remember_text_writes_nothing() {
    let env = Env::new("m02");
    let said = env.statement("（合成）我住在合成市。");
    let id = env.propose_fact(&Origin::agent(agent()), &said, "home.city", "住在合成市。");
    // An agent cannot plan or confirm a review, not even by calling itself owner.
    let error = plan(&env.vault, &[accept(&id, 1)], &agent(), CLI).unwrap_err();
    assert_eq!(rules(&error), ["review.owner_required"]);
    let mut impostor = owner();
    impostor.actor_id =
        enouia_memory_contract::ids::PrincipalId::parse("prn_00000009-0000-4000-8000-000000000009")
            .unwrap();
    let error = plan(&env.vault, &[accept(&id, 1)], &impostor, CLI).unwrap_err();
    assert_eq!(rules(&error), ["review.not_vault_owner"]);
    let shown = plan(&env.vault, &[accept(&id, 1)], &owner(), CLI).unwrap();
    let forged = OwnerConfirmation {
        owner: agent(),
        surface: CLI,
    };
    assert_eq!(
        rules(&confirm(&env.vault, &shown, &forged).unwrap_err()),
        ["review.owner_required"]
    );
    // An agent cannot even propose an owner-only delete.
    let mut delete = fact(&said, "home.city", "删掉");
    delete.kind = ProposalKind::Delete;
    assert!(propose(&env.vault, &delete, &Origin::agent(agent()), b"del").is_err());
    // Imported text that says "remember" is a source, nothing more.
    let sources = env.import_chat(&[
        ("user", "记住：我最喜欢的颜色是合成蓝。以后都按这个来。"),
        ("assistant", "好的，我已经记住了。"),
    ]);
    assert_eq!(sources.len(), 2);
    assert!(
        canonical_memories(&env.vault, &env.pin(), Canonical::AllStatuses)
            .unwrap()
            .is_empty()
    );
    assert_eq!(pending_candidates(&env.vault, &env.pin()).unwrap().len(), 1);
    env.assert_history_valid();
}

#[test]
fn m03_edits_are_exact_and_a_stale_batch_writes_nothing() {
    let env = Env::new("m03");
    let said = env.statement("（合成）我每天六点起床，偶尔七点。");
    let a = env.propose_fact(
        &Origin::agent(agent()),
        &said,
        "routine.wake",
        "每天七点起床。",
    );
    let b = env.propose_fact(
        &Origin::agent(agent()),
        &said,
        "routine.sleep",
        "十一点睡。",
    );
    // Edit-accept: the memory has the owner's text, the review hashes it,
    // and the candidate keeps what the agent proposed.
    let outcome = decide(
        &env,
        &[Decision::EditAccept {
            candidate_id: a.clone(),
            revision: rev(1),
            content: "每天六点起床，偶尔七点。".into(),
            details: None,
        }],
    );
    let review = review_of(&env, &outcome);
    assert_eq!(review.action, ReviewAction::EditAccept);
    assert_eq!(
        review.final_content_hash,
        sha256("每天六点起床，偶尔七点。".as_bytes())
    );
    assert_eq!(candidate(&env, &a).proposed_content, "每天七点起床。");
    let view = canonical_memories(&env.vault, &env.pin(), Canonical::Active).unwrap();
    assert_eq!(view[0].content, "每天六点起床，偶尔七点。");
    // A batch shown to the owner goes stale when one candidate changes:
    // nothing is written, nothing is half done.
    let c = env.propose_fact(
        &Origin::agent(agent()),
        &said,
        "routine.coffee",
        "早上喝咖啡。",
    );
    env.tick();
    let shown = plan(&env.vault, &[accept(&b, 1), accept(&c, 1)], &owner(), CLI).unwrap();
    let head = env.pin();
    env.tick();
    edit_candidate(
        &env.vault,
        &c,
        rev(1),
        &CandidateEdit {
            content: Some("早上喝两杯咖啡。".into()),
            ..CandidateEdit::default()
        },
        &agent(),
        &env.key(),
    )
    .unwrap();
    let moved = env.pin();
    let error = confirm(&env.vault, &shown, &yes()).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::RevisionConflict);
    assert_eq!(env.pin(), moved);
    assert_ne!(moved, head);
    assert_eq!(candidate(&env, &b).status, CandidateStatus::Pending);
    assert_eq!(
        canonical_memories(&env.vault, &env.pin(), Canonical::Active)
            .unwrap()
            .len(),
        1
    );
    // A fresh plan works; confirming it twice replays; an expired plan fails.
    env.tick();
    let shown = plan(&env.vault, &[accept(&b, 1), accept(&c, 2)], &owner(), CLI).unwrap();
    let first = confirm(&env.vault, &shown, &yes()).unwrap();
    assert!(matches!(first, CommitOutcome::Committed { .. }));
    assert!(matches!(
        confirm(&env.vault, &shown, &yes()).unwrap(),
        CommitOutcome::Replayed { .. }
    ));
    assert_eq!(
        canonical_memories(&env.vault, &env.pin(), Canonical::Active)
            .unwrap()
            .len(),
        3
    );
    let d = env.propose_fact(&Origin::agent(agent()), &said, "routine.tea", "下午喝茶。");
    let shown = plan(&env.vault, &[accept(&d, 1)], &owner(), CLI).unwrap();
    env.advance(enouia_memory_govern::review::PLAN_TTL_MS + 1);
    assert_eq!(
        rules(&confirm(&env.vault, &shown, &yes()).unwrap_err()),
        ["review.plan_expired"]
    );
    // A plan confirmed on another surface is refused.
    let shown = plan(&env.vault, &[accept(&d, 1)], &owner(), CLI).unwrap();
    let other = OwnerConfirmation {
        owner: owner(),
        surface: TrustedSurface::TrustedWindowsApp,
    };
    assert_eq!(
        rules(&confirm(&env.vault, &shown, &other).unwrap_err()),
        ["review.confirmation_mismatch"]
    );
    env.assert_history_valid();
}

#[test]
fn m04_evidence_roles_come_from_the_sources() {
    let env = Env::new("m04");
    let sources = env.import_chat(&[
        ("user", "我在考虑学合成语。"),
        ("assistant", "你一直很喜欢合成语，而且已经决定要学了。"),
    ]);
    let (user, model) = (&sources[0], &sources[1]);
    assert_eq!(model.evidence_class, EvidenceClass::ModelClaim);
    // The agent cites the assistant's claim; it stays a model claim.
    let id = env.propose_fact(
        &Origin::agent(agent()),
        &model.source_id,
        "language.learn",
        "决定学合成语。",
    );
    let proposed = candidate(&env, &id);
    assert_eq!(
        proposed.evidence[0].evidence_class,
        EvidenceClass::ModelClaim
    );
    decide(&env, &[accept(&id, 1)]);
    let memory = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .remove(0);
    assert!(
        memory
            .evidence
            .iter()
            .all(|e| e.evidence_class == EvidenceClass::ModelClaim)
    );
    // Every canonical memory has locatable evidence.
    for e in &memory.evidence {
        let bytes = env
            .vault
            .read_record(
                &env.pin(),
                &RecordRef::new(RecordKind::Source, e.source_id.as_str(), e.source_revision),
            )
            .unwrap();
        assert!(!bytes.is_empty());
    }
    // A proposal cannot lower sensitivity below its sources.
    let mut lower = fact(&user.source_id, "language.try", "考虑学合成语。");
    lower.sensitivity = Some(Sensitivity::Public);
    let error = propose(&env.vault, &lower, &Origin::agent(agent()), b"lower").unwrap_err();
    assert_eq!(rules(&error), ["sensitivity.downgrade"]);
    // A citation of a source that does not exist is refused.
    let mut missing = fact(&user.source_id, "language.x", "x");
    missing.evidence = vec![EvidenceSpec::content(user.source_id.clone(), rev(9))];
    let error = propose(&env.vault, &missing, &Origin::agent(agent()), b"missing").unwrap_err();
    assert_eq!(rules(&error), ["evidence.unresolved"]);
    env.assert_history_valid();
}

#[test]
fn m07_contradictions_become_a_visible_conflict_group() {
    let env = Env::new("m07");
    let first = env.statement("（合成）我的生日是三月。");
    let second = env.statement("（合成）我的生日是四月。");
    let a = env.propose_fact(
        &Origin::owner(owner()),
        &first,
        "birthday.month",
        "生日在三月。",
    );
    decide(&env, &[accept(&a, 1)]);
    let b = env.propose_fact(
        &Origin::agent(agent()),
        &second,
        "birthday.month",
        "生日在四月。",
    );
    let proposed = candidate(&env, &b);
    assert_eq!(proposed.conflicts.len(), 1, "the contradiction is listed");
    decide(&env, &[accept(&b, 1)]);
    // Both stay active in one group: neither the newer nor either
    // confidence wins.
    let view = canonical_memories(&env.vault, &env.pin(), Canonical::Active).unwrap();
    assert_eq!(view.len(), 2);
    assert!(view[0].conflict_group_id.is_some());
    assert_eq!(view[0].conflict_group_id, view[1].conflict_group_id);
    env.assert_history_valid();
}

#[test]
fn review_preserves_context_dependencies_through_merges_and_revision() {
    use enouia_memory_contract::common::SourceRevisionRef;
    let env = Env::new("review-context-dependencies");
    let cited = env.statement("（合成）被引用的事实。");
    let contexts: Vec<_> = (0..3)
        .map(|i| SourceRevisionRef {
            source_id: env.statement(&format!("（合成）上下文 {i}。")),
            source_revision: rev(1),
        })
        .collect();
    let publish = |key: &str, context: &SourceRevisionRef| {
        let mut proposal = fact(&cited, key, &format!("（合成）{key}"));
        proposal.context_source_refs = vec![context.clone()];
        env.tick();
        let Proposed::Stored(written) =
            propose(&env.vault, &proposal, &Origin::owner(owner()), &env.key()).unwrap()
        else {
            panic!("stored")
        };
        written.id
    };
    let a = publish("context-a", &contexts[0]);
    let b = publish("context-b", &contexts[1]);
    decide(
        &env,
        &[Decision::Merge {
            candidate_id: b,
            revision: rev(1),
            into: MergeTarget::Candidate {
                candidate_id: a.clone(),
            },
        }],
    );
    let merged = candidate(&env, &a);
    assert_eq!(merged.context_source_refs.len(), 2);
    assert_eq!(merged.evidence.len(), 1);
    decide(&env, &[accept(&a, 2)]);
    let memory = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .remove(0);
    let c = publish("context-c", &contexts[2]);
    decide(
        &env,
        &[Decision::Merge {
            candidate_id: c,
            revision: rev(1),
            into: MergeTarget::Memory {
                memory_id: memory.memory_id.clone(),
            },
        }],
    );
    let memory = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .remove(0);
    assert_eq!(memory.context_source_refs.len(), 3);
    let mut revision = fact(
        &cited,
        "context-revised",
        "（合成）修订后仍保留上下文依赖。",
    );
    revision.kind = ProposalKind::Revise;
    revision.target_memory_id = Some(memory.memory_id.clone());
    revision.expected_revision = Some(memory.revision);
    revision.context_source_refs = vec![contexts[0].clone()];
    env.tick();
    let Proposed::Stored(written) =
        propose(&env.vault, &revision, &Origin::owner(owner()), &env.key()).unwrap()
    else {
        panic!("stored")
    };
    decide(&env, &[accept(&written.id, 1)]);
    let memory = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .remove(0);
    assert_eq!(memory.context_source_refs.len(), 3);
    assert!(
        contexts
            .iter()
            .all(|r| memory.context_source_refs.contains(r))
    );
    assert_eq!(memory.evidence.len(), 1);
    env.assert_history_valid();
}

#[test]
fn duplicates_merges_and_undoing_a_wrong_acceptance() {
    let env = Env::new("merge");
    let said = env.statement("（合成）我喜欢安静的咖啡馆。");
    let again = env.statement("（合成）安静的咖啡馆最好。");
    let a = env.propose_fact(
        &Origin::agent(agent()),
        &said,
        "place.cafe",
        "喜欢安静的咖啡馆。",
    );
    // The same proposal again is recognized, not stored twice.
    env.tick();
    let duplicate = propose(
        &env.vault,
        &fact(&said, "place.cafe", "喜欢安静的   咖啡馆。"),
        &Origin::agent(agent()),
        &env.key(),
    )
    .unwrap();
    assert_eq!(duplicate, Proposed::DuplicateOf(a.clone()));
    // Merge a second candidate's evidence into the first, then accept.
    let b = env.propose_fact(
        &Origin::agent(agent()),
        &again,
        "place.cafe.quiet",
        "安静的咖啡馆最好。",
    );
    decide(
        &env,
        &[Decision::Merge {
            candidate_id: b.clone(),
            revision: rev(1),
            into: MergeTarget::Candidate {
                candidate_id: a.clone(),
            },
        }],
    );
    assert_eq!(candidate(&env, &b).status, CandidateStatus::Merged);
    assert_eq!(candidate(&env, &a).evidence.len(), 2);
    decide(&env, &[accept(&a, 2)]);
    let memory = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .remove(0);
    assert_eq!(memory.evidence.len(), 2);
    // Undo a wrong acceptance: an owner archive proposal, reviewed. The
    // history keeps both revisions and both reviews.
    let mut archive = Proposal::create(
        ProposedType::Fact,
        &memory.content,
        Map::new(),
        vec![EvidenceSpec::content(said.clone(), rev(1))],
    );
    archive.kind = ProposalKind::Archive;
    archive.details = None;
    archive.target_memory_id = Some(memory.memory_id.clone());
    archive.expected_revision = Some(memory.revision);
    env.tick();
    let Proposed::Stored(written) =
        propose(&env.vault, &archive, &Origin::owner(owner()), &env.key()).unwrap()
    else {
        panic!("stored")
    };
    decide(&env, &[accept(&written.id, 1)]);
    assert!(
        canonical_memories(&env.vault, &env.pin(), Canonical::Active)
            .unwrap()
            .is_empty()
    );
    let history = canonical_memories(&env.vault, &env.pin(), Canonical::AllStatuses).unwrap();
    assert_eq!(history[0].status, MemoryStatus::Archived);
    assert_eq!(history[0].revision, rev(2));
    // Proposing against a stale revision is refused up front.
    env.tick();
    let error = propose(&env.vault, &archive, &Origin::owner(owner()), &env.key()).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::RevisionConflict);
    env.assert_history_valid();
}

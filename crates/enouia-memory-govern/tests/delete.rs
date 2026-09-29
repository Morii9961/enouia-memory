//! MV-3.4 acceptance: forgetting is an immediate barrier (P01), a purge is
//! previewed, removes exactly what it names, and ends in a receipt that
//! claims no global erasure (P01), and a restored old backup is reconciled
//! with the deletion ledger before its network gate opens (B02).

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::candidate::{ProposalKind, ReviewAction};
use enouia_memory_contract::commit::{
    DeleteMode, DeleteScope, OperationKind, PurgeOverall, PurgeReceipt, StoreKind, StorePurgeState,
};
use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::ids::CandidateId;
use enouia_memory_contract::memory::CanonicalMemory;
use enouia_memory_contract::ports::{CommitOutcome, SequentialIdSource};
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_govern::delete::{
    complete_purge, delete_proposal, deletion_ledger, ledger_bytes, parse_ledger, purge_preview,
    reconcile_deletions,
};
use enouia_memory_govern::{
    Canonical, Decision, Origin, OwnerConfirmation, Proposed, ReviewPlan, canonical_memories,
    confirm, plan, propose,
};
use enouia_memory_vault::backup::{export_pinned, network_allowed, restore_export, verify_export};
use enouia_memory_vault::{Fault, RootPolicy, VaultOptions, verify_data_root};
use std::path::Path;
use std::sync::Arc;
use support::{Env, agent, owner, rev};

const CLI: TrustedSurface = TrustedSurface::TrustedLocalCli;

fn yes() -> OwnerConfirmation {
    OwnerConfirmation {
        owner: owner(),
        surface: CLI,
    }
}

fn accept_new(env: &Env, id: &CandidateId) -> CanonicalMemory {
    env.tick();
    let shown = plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: id.clone(),
            revision: rev(1),
        }],
        &owner(),
        CLI,
    )
    .unwrap();
    confirm(&env.vault, &shown, &yes()).unwrap();
    let written = &shown.ids[0].memory_id;
    canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .into_iter()
        .find(|m| &m.memory_id == written)
        .unwrap()
}

/// Propose and plan the owner's deletion of `memory`.
fn delete_plan(
    env: &Env,
    memory: &CanonicalMemory,
    mode: DeleteMode,
    scope: DeleteScope,
) -> ReviewPlan {
    env.tick();
    let Proposed::Stored(written) = propose(
        &env.vault,
        &delete_proposal(memory, mode, scope),
        &Origin::owner(owner()),
        &env.key(),
    )
    .unwrap() else {
        panic!("stored")
    };
    env.tick();
    plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: written.id,
            revision: rev(1),
        }],
        &owner(),
        CLI,
    )
    .unwrap()
}

/// Every file under `dir` whose bytes contain `needle`.
fn files_containing(dir: &Path, needle: &str, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_containing(&path, needle, hits);
        } else if String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(needle) {
            hits.push(path.display().to_string());
        }
    }
}

#[test]
fn p01_forgetting_is_owner_only_and_an_immediate_barrier() {
    let env = Env::new("forget");
    let said = env.statement("（合成）我以前的邮编是 100000。");
    let id = env.propose_fact(
        &Origin::owner(owner()),
        &said,
        "address.postcode",
        "邮编 100000。",
    );
    let memory = accept_new(&env, &id);
    let before = env.pin();
    // An agent cannot ask to delete; only the owner can.
    let mut request = delete_proposal(
        &memory,
        DeleteMode::LogicalDelete,
        DeleteScope::AllRevisions,
    );
    request.reason = "agent".into();
    let error = propose(
        &env.vault,
        &request,
        &Origin::agent(agent()),
        b"agent-delete",
    )
    .unwrap_err();
    assert!(
        matches!(&error.fault, Fault::Contract(r) if r.contains(&"candidate.delete_owner_only"))
    );
    let shown = delete_plan(
        &env,
        &memory,
        DeleteMode::LogicalDelete,
        DeleteScope::AllRevisions,
    );
    assert_eq!(shown.operation_kind, OperationKind::LogicalDelete);
    let tombstone = shown.diff["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["record_kind"] == "tombstone")
        .unwrap();
    assert_eq!(
        tombstone["value"]["targets"][0]["record_id"],
        memory.memory_id.as_str()
    );
    confirm(&env.vault, &shown, &yes()).unwrap();
    // Gone from the canonical view at once; a read pinned before it is
    // refused before content leaves; the text itself is kept, restricted.
    assert!(
        canonical_memories(&env.vault, &env.pin(), Canonical::AllStatuses)
            .unwrap()
            .is_empty()
    );
    let reference = RecordRef::new(
        RecordKind::Memory,
        memory.memory_id.as_str(),
        memory.revision,
    );
    let error = env
        .vault
        .check_fresh(&before, std::slice::from_ref(&reference))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::NotFound);
    assert!(env.vault.read_revision(&env.pin(), &reference).is_ok());
    assert!(env.vault.verify(&env.pin()).unwrap().is_clean());
    // A delete confirmation never rides inside a batch.
    let other = env.propose_fact(&Origin::owner(owner()), &said, "address.old", "旧地址。");
    let kept = accept_new(&env, &other);
    env.tick();
    let Proposed::Stored(delete) = propose(
        &env.vault,
        &delete_proposal(&kept, DeleteMode::LogicalDelete, DeleteScope::AllRevisions),
        &Origin::owner(owner()),
        &env.key(),
    )
    .unwrap() else {
        panic!("stored")
    };
    let third = env.propose_fact(&Origin::owner(owner()), &said, "address.city", "合成市。");
    let error = plan(
        &env.vault,
        &[
            Decision::Accept {
                candidate_id: delete.id,
                revision: rev(1),
            },
            Decision::Accept {
                candidate_id: third,
                revision: rev(1),
            },
        ],
        &owner(),
        CLI,
    )
    .unwrap_err();
    assert!(matches!(&error.fault, Fault::Contract(r) if r == &["review.delete_alone"]));
    env.assert_history_valid();
}

#[test]
fn p01_a_purge_is_previewed_removes_what_it_names_and_claims_no_global_erasure() {
    let env = Env::new("purge");
    const SECRET: &str = "合成路1号三单元";
    let sources = env.import_chat(&[
        ("user", &format!("我家住在{SECRET}。")),
        ("assistant", "好的。"),
        ("user", "我喜欢喝乌龙茶。"),
    ]);
    let (home, tea) = (&sources[0], &sources[2]);
    let a = env.propose_fact(
        &Origin::owner(owner()),
        &home.source_id,
        "home.address",
        &format!("住在{SECRET}。"),
    );
    let address = accept_new(&env, &a);
    let b = env.propose_fact(
        &Origin::owner(owner()),
        &tea.source_id,
        "drink.tea",
        "喜欢乌龙茶。",
    );
    let tea_memory = accept_new(&env, &b);
    // The preview lists everything, including what shares the raw export.
    let narrow = purge_preview(
        &env.vault,
        &address.memory_id,
        DeleteMode::Purge,
        DeleteScope::AllRevisions,
    )
    .unwrap();
    assert!(narrow.object_hashes.is_empty());
    assert!(narrow.targets.iter().any(|t| t.0 == RecordKind::Candidate));
    let wide = purge_preview(
        &env.vault,
        &address.memory_id,
        DeleteMode::Purge,
        DeleteScope::WithDependents,
    )
    .unwrap();
    let raw = home.raw_object_hash.clone().unwrap();
    assert_eq!(wide.object_hashes, vec![raw.clone()]);
    assert_eq!(
        wide.shared_raw,
        vec![(raw.clone(), 2)],
        "two other messages in the export"
    );
    assert_eq!(wide.losing_provenance, vec![tea_memory.memory_id.clone()]);
    assert!(
        wide.targets
            .iter()
            .any(|t| t.0 == RecordKind::Source && t.1 == home.source_id.as_str())
    );
    // Positive control: before the purge the text is on disk in several
    // places (raw export, memory, candidate).
    let mut before = Vec::new();
    files_containing(env.vault.managed_root().root(), SECRET, &mut before);
    assert!(before.len() >= 3, "{before:?}");
    // The owner confirms exactly that; the tombstone is the barrier.
    let shown = delete_plan(
        &env,
        &address,
        DeleteMode::Purge,
        DeleteScope::WithDependents,
    );
    assert_eq!(shown.operation_kind, OperationKind::Purge);
    let outcome = confirm(&env.vault, &shown, &yes()).unwrap();
    assert!(matches!(outcome, CommitOutcome::Committed { .. }));
    let delete_id = shown.ids[0].delete_id.clone();
    env.tick();
    let done = complete_purge(&env.vault, &delete_id, &owner(), b"purge-1").unwrap();
    assert!(!done.files.removed.is_empty());
    // The content is gone from the Vault and says so on every read.
    let mut hits = Vec::new();
    files_containing(env.vault.managed_root().root(), SECRET, &mut hits);
    assert!(hits.is_empty(), "{hits:?}");
    let pin = env.pin();
    let reference = RecordRef::new(RecordKind::Memory, address.memory_id.as_str(), rev(1));
    let purged =
        |e: enouia_memory_vault::VaultError| e.code() == MemoryErrorCode::IntentionallyPurged;
    assert!(purged(
        env.vault.read_revision(&pin, &reference).unwrap_err()
    ));
    assert!(purged(env.vault.read_object(&pin, &raw).unwrap_err()));
    let report = env.vault.verify(&pin).unwrap();
    assert!(report.is_clean() && report.purged >= 4, "{report:?}");
    // The receipt is honest: local stores purged, backups pending with a
    // deadline, exports outside the Vault, never "globally erased".
    let bytes = env
        .vault
        .read_record(
            &pin,
            &RecordRef::new(RecordKind::PurgeReceipt, done.receipt_id.as_str(), rev(1)),
        )
        .unwrap();
    let receipt: PurgeReceipt = enouia_memory_contract::parse_record(&bytes).unwrap();
    assert_eq!(receipt.overall_state, PurgeOverall::BackupPurgePending);
    assert!(receipt.latest_purge_deadline.is_some());
    let state = |s: StoreKind| receipt.stores.iter().find(|x| x.store == s).unwrap().state;
    assert_eq!(state(StoreKind::Canonical), StorePurgeState::Purged);
    assert_eq!(state(StoreKind::Raw), StorePurgeState::Purged);
    assert_eq!(state(StoreKind::Candidates), StorePurgeState::Purged);
    assert_eq!(state(StoreKind::Backups), StorePurgeState::Pending);
    assert_eq!(state(StoreKind::Exports), StorePurgeState::NotManageable);
    // Repeating the purge is harmless and replays the receipt.
    let again = complete_purge(&env.vault, &delete_id, &owner(), b"purge-1").unwrap();
    assert!(again.files.removed.is_empty());
    assert!(matches!(again.commit, CommitOutcome::Replayed { .. }));
    // The Vault keeps working: exports verify and carry no purged text,
    // sweeps quarantine nothing, and later commits validate.
    let out = env.root.join("export");
    std::fs::create_dir_all(&out).unwrap();
    let destination = verify_data_root(&out, &RootPolicy::default()).unwrap();
    export_pinned(&env.vault, &env.pin(), &destination).unwrap();
    verify_export(&out).unwrap();
    let mut hits = Vec::new();
    files_containing(&out, SECRET, &mut hits);
    assert!(hits.is_empty(), "{hits:?}");
    assert!(
        env.vault
            .sweep_unreferenced()
            .unwrap()
            .quarantined
            .is_empty()
    );
    let later = env.statement("（合成）我也喜欢绿茶。");
    let c = env.propose_fact(
        &Origin::owner(owner()),
        &later,
        "drink.green",
        "也喜欢绿茶。",
    );
    accept_new(&env, &c);
    // The tea memory stays; its raw evidence is gone and reads say so.
    let tea_now = canonical_memories(&env.vault, &env.pin(), Canonical::Active)
        .unwrap()
        .into_iter()
        .find(|m| m.memory_id == tea_memory.memory_id)
        .unwrap();
    assert_eq!(tea_now.evidence[0].object_hash, raw);
    let review = env
        .vault
        .record_entries(&env.pin(), RecordKind::Review)
        .unwrap();
    assert!(review.len() >= 3);
    let _ = (ProposalKind::Delete, ReviewAction::ConfirmDelete);
}

#[test]
fn b02_a_restored_old_backup_is_reconciled_before_its_network_gate_opens() {
    let env = Env::new("b02");
    let said = env.statement("（合成）前任的名字是合成名。");
    let id = env.propose_fact(
        &Origin::owner(owner()),
        &said,
        "relationship.former",
        "前任叫合成名。",
    );
    let memory = accept_new(&env, &id);
    // An export taken before the owner forgets it.
    let old = env.root.join("old-export");
    std::fs::create_dir_all(&old).unwrap();
    export_pinned(
        &env.vault,
        &env.pin(),
        &verify_data_root(&old, &RootPolicy::default()).unwrap(),
    )
    .unwrap();
    let shown = delete_plan(
        &env,
        &memory,
        DeleteMode::LogicalDelete,
        DeleteScope::AllRevisions,
    );
    confirm(&env.vault, &shown, &yes()).unwrap();
    // The ledger holds IDs and modes only, kept apart from the backups.
    let ledger = ledger_bytes(&deletion_ledger(&env.vault).unwrap());
    assert!(!String::from_utf8_lossy(&ledger).contains("合成名"));
    let mut control = Vec::new();
    files_containing(env.vault.managed_root().root(), "合成名", &mut control);
    assert!(
        !control.is_empty(),
        "the text is stored as UTF-8, so the check above can fail"
    );
    // Restoring the old export brings the memory back, with the network
    // gate closed until reconciliation.
    let target = env.root.join("restored");
    std::fs::create_dir_all(&target).unwrap();
    env.tick();
    let restored = restore_export(
        &old,
        &verify_data_root(&target, &RootPolicy::default()).unwrap(),
        &owner(),
        CLI,
        env.clock.clone(),
        Arc::new(SequentialIdSource::new(0xb000)),
        VaultOptions::default(),
    )
    .unwrap();
    let vault = &restored.vault;
    assert!(!network_allowed(vault).unwrap());
    let pin = vault.pin_current().unwrap();
    assert_eq!(
        canonical_memories(vault, &pin, Canonical::Active)
            .unwrap()
            .len(),
        1
    );
    // Reconciliation re-applies the deletion, then opens the gate.
    env.tick();
    let result = reconcile_deletions(vault, &parse_ledger(&ledger).unwrap(), &yes()).unwrap();
    assert_eq!(result.applied.len(), 1);
    let pin = vault.pin_current().unwrap();
    assert!(
        canonical_memories(vault, &pin, Canonical::Active)
            .unwrap()
            .is_empty()
    );
    assert!(network_allowed(vault).unwrap());
    // Running it again finds nothing left to do.
    env.tick();
    let again = reconcile_deletions(vault, &parse_ledger(&ledger).unwrap(), &yes()).unwrap();
    assert!(again.applied.is_empty() && again.already.len() == 1);
}

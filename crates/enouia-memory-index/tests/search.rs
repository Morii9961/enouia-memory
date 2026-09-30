//! MV-4 acceptance: the golden queries (R02, R03, R06 literal input), a
//! deleted, corrupted, or rebuilt index changes nothing (R01), permissions
//! filter before anything is returned (R04), a lagging index is never mixed
//! with newer records (R05), pages are bound to their snapshot (R06),
//! `known_at` reads the past, and a purge leaves no text in the index file.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::{DeleteMode, DeleteScope};
use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::memory::CanonicalMemory;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_govern::delete::{complete_purge, delete_proposal};
use enouia_memory_govern::{Decision, Origin, OwnerConfirmation, Proposed, confirm, plan, propose};
use enouia_memory_index::{Index, SearchPage, SearchRequest, search};
use enouia_memory_vault::{Fault, VaultError};
use std::collections::BTreeSet;
use support::{Env, agent, golden, owner};

fn run(env: &Env, index: &Index, request: &SearchRequest) -> SearchPage {
    search(index, &env.vault, &owner(), request).unwrap()
}

fn labels(env: &Env, page: &SearchPage) -> Vec<String> {
    page.items
        .iter()
        .map(|h| env.label_of(&h.memory_id))
        .collect()
}

fn rule(error: &VaultError) -> Vec<&'static str> {
    match &error.fault {
        Fault::Contract(rules) => rules.clone(),
        _ => Vec::new(),
    }
}

fn check_golden(env: &Env, index: &Index) {
    for case in golden()["queries"].as_array().unwrap() {
        let query = case["query"].as_str().unwrap();
        let page = run(env, index, &SearchRequest::text(query));
        let got = labels(env, &page);
        if let Some(expect) = case.get("expect") {
            let want: Vec<String> = serde_json::from_value(expect.clone()).unwrap();
            assert_eq!(got, want, "query {query:?}");
        }
        if let Some(expect) = case.get("expect_set") {
            let want: BTreeSet<String> = serde_json::from_value(expect.clone()).unwrap();
            assert_eq!(
                got.into_iter().collect::<BTreeSet<_>>(),
                want,
                "query {query:?}"
            );
        }
        if let Some(expect) = case.get("projects") {
            let want: BTreeSet<String> = serde_json::from_value(expect.clone()).unwrap();
            let names: BTreeSet<String> = page
                .projects
                .iter()
                .map(|p| p.display_name.clone())
                .collect();
            assert_eq!(names, want, "projects for {query:?}");
        }
        assert!(!page.partial, "{query:?}");
        // The IPC shape round-trips through the contract type.
        let ipc = page.to_ipc();
        let value = serde_json::to_value(&ipc).unwrap();
        let back: enouia_memory_contract::ipc::MemorySearchResult =
            serde_json::from_value(value).unwrap();
        assert_eq!(back, ipc);
        for hit in &page.items {
            assert!(!hit.snippet.is_empty());
        }
    }
}

#[test]
fn golden_queries_hold_and_a_rebuilt_index_answers_the_same() {
    let mut env = Env::new("golden");
    env.golden_corpus();
    let mut index = Index::open(&env.vault).unwrap();
    let report = index.update(&env.vault, &|| false).unwrap();
    assert!(report.reached_head && report.revisions_indexed >= 12);
    check_golden(&env, &index);
    // "mori" names two projects; neither is chosen for the owner.
    let page = run(&env, &index, &SearchRequest::text("mori"));
    assert!(page.projects.iter().all(|p| !p.exact));
    // Rebuilding from the Vault gives the same answers (R01).
    drop(index);
    let (index, _) = Index::rebuild(&env.vault).unwrap();
    check_golden(&env, &index);
}

#[test]
fn r01_a_deleted_or_corrupted_index_is_rebuilt_without_loss() {
    let mut env = Env::new("r01");
    env.golden_corpus();
    let (index, _) = Index::rebuild(&env.vault).unwrap();
    let path = index.path().to_path_buf();
    let before = labels(&env, &run(&env, &index, &SearchRequest::text("记忆")));
    drop(index);
    // Deleted: an empty index is not ready until it catches up.
    std::fs::remove_file(&path).unwrap();
    let mut index = Index::open(&env.vault).unwrap();
    let error = search(&index, &env.vault, &owner(), &SearchRequest::text("记忆")).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::IndexNotReady);
    index.update(&env.vault, &|| false).unwrap();
    assert_eq!(
        labels(&env, &run(&env, &index, &SearchRequest::text("记忆"))),
        before
    );
    drop(index);
    // Corrupted: refused, then rebuilt; the Vault was never touched.
    std::fs::write(&path, b"not a sqlite database at all, synthetic damage").unwrap();
    let error = Index::open(&env.vault).err().unwrap();
    assert_eq!(error.code(), MemoryErrorCode::IndexNotReady);
    assert!(
        env.vault
            .verify(&env.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
    let (index, _) = Index::rebuild(&env.vault).unwrap();
    assert_eq!(
        labels(&env, &run(&env, &index, &SearchRequest::text("记忆"))),
        before
    );
}

#[test]
fn r04_permissions_apply_before_snippets_counts_projects_or_cursors() {
    let mut env = Env::new("r04");
    env.golden_corpus();
    let (mut index, _) = Index::rebuild(&env.vault).unwrap();
    // The genesis policy grants reads to the owner only.
    for query in ["MoriMeta", "记忆", "mori", "的"] {
        let page = search(&index, &env.vault, &agent(), &SearchRequest::text(query)).unwrap();
        assert!(page.items.is_empty(), "{query}");
        assert!(page.projects.is_empty(), "{query}: project names leak");
        assert!(page.next_cursor.is_none() && !page.partial, "{query}");
    }
    // A forgotten memory is gone from every search at once.
    let memory = env.labels["g04"].clone();
    let pin = env.vault.pin_current().unwrap();
    let bytes = env
        .vault
        .read_record(
            &pin,
            &RecordRef::new(
                RecordKind::Memory,
                memory.as_str(),
                Revision::new(1).unwrap(),
            ),
        )
        .unwrap();
    let record: CanonicalMemory = enouia_memory_contract::parse_record(&bytes).unwrap();
    forget(&env, &record, DeleteMode::LogicalDelete);
    index.update(&env.vault, &|| false).unwrap();
    assert!(
        run(&env, &index, &SearchRequest::text("琉璃光院"))
            .items
            .is_empty()
    );
}

fn forget(
    env: &Env,
    memory: &CanonicalMemory,
    mode: DeleteMode,
) -> enouia_memory_govern::ReviewPlan {
    env.tick();
    let Proposed::Stored(written) = propose(
        &env.vault,
        &delete_proposal(memory, mode, DeleteScope::AllRevisions),
        &Origin::owner(owner()),
        format!("forget-{}", memory.memory_id).as_bytes(),
    )
    .unwrap() else {
        panic!("stored")
    };
    env.tick();
    let shown = plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: written.id,
            revision: Revision::new(1).unwrap(),
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    confirm(
        &env.vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
    shown
}

#[test]
fn r05_a_lagging_index_says_so_and_never_mixes_versions() {
    let mut env = Env::new("r05");
    env.golden_corpus();
    let (mut index, _) = Index::rebuild(&env.vault).unwrap();
    env.remember("g13", "hobby.hiking", "周末去爬山，带上记忆卡。", None);
    let error = search(&index, &env.vault, &owner(), &SearchRequest::text("记忆")).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::IndexNotReady);
    // Cancelled before any commit: still behind, still refused.
    let report = index.update(&env.vault, &|| true).unwrap();
    assert!(!report.reached_head && report.commits_applied == 0);
    assert!(search(&index, &env.vault, &owner(), &SearchRequest::text("记忆")).is_err());
    index.update(&env.vault, &|| false).unwrap();
    let got = labels(&env, &run(&env, &index, &SearchRequest::text("记忆")));
    assert!(got.contains(&"g13".to_owned()));
}

#[test]
fn r06_pages_are_bound_to_their_snapshot_filters_and_epochs() {
    let mut env = Env::new("r06");
    env.golden_corpus();
    let (mut index, _) = Index::rebuild(&env.vault).unwrap();
    let full = labels(
        &env,
        &run(
            &env,
            &index,
            &SearchRequest {
                limit: Some(100),
                ..SearchRequest::text("的")
            },
        ),
    );
    assert!(full.len() >= 5, "{full:?}");
    let request = SearchRequest {
        limit: Some(2),
        ..SearchRequest::text("的")
    };
    let first = run(&env, &index, &request);
    let cursor = first.next_cursor.clone().unwrap();
    // New commits after the first page do not shift the next pages.
    env.remember("g13", "notes.misc", "今天的笔记很短。", None);
    index.update(&env.vault, &|| false).unwrap();
    let mut pages = labels(&env, &first);
    let mut next = Some(cursor.clone());
    while let Some(cursor) = next {
        let page = run(
            &env,
            &index,
            &SearchRequest {
                cursor: Some(cursor),
                ..request.clone()
            },
        );
        pages.extend(labels(&env, &page));
        next = page.next_cursor;
    }
    assert_eq!(pages, full, "pages follow the first snapshot");
    // A cursor is bound to its query, principal, and integrity.
    let other = search(
        &index,
        &env.vault,
        &owner(),
        &SearchRequest {
            cursor: Some(cursor.clone()),
            ..SearchRequest::text("记忆")
        },
    )
    .unwrap_err();
    assert_eq!(rule(&other), ["search.cursor_stale"]);
    let agent_page = search(
        &index,
        &env.vault,
        &agent(),
        &SearchRequest {
            cursor: Some(cursor.clone()),
            ..request.clone()
        },
    )
    .unwrap_err();
    assert_eq!(rule(&agent_page), ["search.cursor_stale"]);
    let mut forged = cursor.clone();
    forged.replace_range(0..2, "ff");
    let error = run_err(
        &env,
        &index,
        &SearchRequest {
            cursor: Some(forged),
            ..request.clone()
        },
    );
    assert_eq!(rule(&error), ["search.cursor_invalid"]);
    // A deletion moves the epoch: old cursors are refused.
    let pin = env.vault.pin_current().unwrap();
    let id = env.labels["g06"].clone();
    let bytes = env
        .vault
        .read_record(
            &pin,
            &RecordRef::new(RecordKind::Memory, id.as_str(), Revision::new(1).unwrap()),
        )
        .unwrap();
    forget(
        &env,
        &enouia_memory_contract::parse_record(&bytes).unwrap(),
        DeleteMode::LogicalDelete,
    );
    index.update(&env.vault, &|| false).unwrap();
    let error = run_err(
        &env,
        &index,
        &SearchRequest {
            cursor: Some(cursor),
            ..request.clone()
        },
    );
    assert_eq!(rule(&error), ["search.cursor_stale"]);
    // Limits and empty queries are refused, not widened.
    assert_eq!(
        rule(&run_err(&env, &index, &SearchRequest::text("   "))),
        ["search.empty_query"]
    );
    assert_eq!(
        rule(&run_err(
            &env,
            &index,
            &SearchRequest {
                limit: Some(101),
                ..SearchRequest::text("的")
            }
        )),
        ["search.limit"]
    );
}

fn run_err(env: &Env, index: &Index, request: &SearchRequest) -> VaultError {
    search(index, &env.vault, &owner(), request).unwrap_err()
}

#[test]
fn known_at_reads_the_past_and_a_purge_leaves_no_text_in_the_index() {
    let mut env = Env::new("history");
    env.golden_corpus();
    let (mut index, _) = Index::rebuild(&env.vault).unwrap();
    // Before g12 was approved, the system did not know it.
    let before_g12 = {
        let pin = env.vault.pin_current().unwrap();
        let head = env.vault.stored_commit(&pin).unwrap();
        head.parent_commit_id.clone().unwrap()
    };
    let now = labels(&env, &run(&env, &index, &SearchRequest::text("茶")));
    assert_eq!(now, ["g06", "g12"]);
    let then = labels(
        &env,
        &run(
            &env,
            &index,
            &SearchRequest {
                known_at: Some(before_g12),
                ..SearchRequest::text("茶")
            },
        ),
    );
    assert_eq!(then, ["g06"]);
    // Positive control: the text is in the index file before the purge.
    let path = index.path().to_path_buf();
    let secret = "乌龙茶".as_bytes();
    let contains = |bytes: &[u8]| bytes.windows(secret.len()).any(|w| w == secret);
    assert!(contains(&std::fs::read(&path).unwrap()));
    let pin = env.vault.pin_current().unwrap();
    let id = env.labels["g06"].clone();
    let bytes = env
        .vault
        .read_record(
            &pin,
            &RecordRef::new(RecordKind::Memory, id.as_str(), Revision::new(1).unwrap()),
        )
        .unwrap();
    let shown = forget(
        &env,
        &enouia_memory_contract::parse_record(&bytes).unwrap(),
        DeleteMode::Purge,
    );
    env.tick();
    complete_purge(&env.vault, &shown.ids[0].delete_id, &owner(), b"purge").unwrap();
    let report = index.update(&env.vault, &|| false).unwrap();
    assert!(report.rows_purged >= 1);
    assert!(
        !contains(&std::fs::read(&path).unwrap()),
        "purged text left in the index file"
    );
    assert_eq!(
        labels(&env, &run(&env, &index, &SearchRequest::text("茶"))),
        ["g12"]
    );
    // A rebuild never reads it back either.
    drop(index);
    let (index, _) = Index::rebuild(&env.vault).unwrap();
    assert!(!contains(&std::fs::read(index.path()).unwrap()));
}

#[test]
fn as_of_hides_what_is_not_yet_in_effect_and_shows_it_when_asked() {
    let mut env = Env::new("as-of");
    let month = 30 * 24 * 3600 * 1000;
    let from = enouia_memory_contract::time::Timestamp::from_unix_ms(support::T0 + month).unwrap();
    let mut extra = serde_json::Map::new();
    extra.insert("valid_from".into(), serde_json::json!(from));
    env.remember_with("f01", "meeting.day", "下个月起每周三开例会。", None, extra);
    let (index, _) = Index::rebuild(&env.vault).unwrap();
    assert!(
        run(&env, &index, &SearchRequest::text("例会"))
            .items
            .is_empty()
    );
    let later =
        enouia_memory_contract::time::Timestamp::from_unix_ms(support::T0 + month + 1).unwrap();
    let page = run(
        &env,
        &index,
        &SearchRequest {
            as_of: Some(later),
            ..SearchRequest::text("例会")
        },
    );
    assert_eq!(labels(&env, &page), ["f01"]);
}

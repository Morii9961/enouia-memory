use super::*;
use enouia_memory_contract::{
    candidate::{CandidateRecord, CandidateStatus},
    memory::MemoryStatus,
    time::{BusinessTime, Timestamp},
};
use enouia_memory_govern::{CandidateEdit, Canonical, Decision, OwnerConfirmation, Proposed};
use enouia_memory_import::{ImportOptions, history::HistoryLimits, import_file};
use enouia_memory_provider::{
    extraction,
    history::{HistoryOptions, HistoryProgress, materialize},
    history_review::{self, BranchRelation, Difference, ReviewFlag, RunSelection},
};

// Streaming segmentation already has dedicated codec tests. One chunk keeps
// these multi-run governance fixtures from writing thousands of ledger deltas.
struct ReviewHttp(Vec<u8>);
impl Transport for ReviewHttp {
    fn exchange(
        &self,
        _: &WireRequest,
        _: &SecretBytes,
        _: &dyn Cancellation,
        _: Duration,
        sink: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        sink(&self.0)
    }
}

fn batches(env: &Env, api: Api, indices: Vec<usize>) -> HistoryProgress {
    let file = env.root.join("synthetic-history.json");
    std::fs::write(
        &file,
        include_bytes!("../../../../tests/fixtures/history/initialization-synthetic.json"),
    )
    .unwrap();
    let mut options = ImportOptions::new(owner(), "synthetic", env.policy());
    options.sensitivity = Sensitivity::Normal;
    let imported = import_file(&env.vault, &file, &options, &NeverCancel).unwrap();
    materialize(
        &env.vault,
        &owner(),
        &HistoryOptions {
            import_id: imported.import_id,
            local_policy: env.policy(),
            history_limits: HistoryLimits {
                messages_per_batch: 2,
                ..HistoryLimits::default()
            },
            batch_indices: indices,
            api,
            model: "synthetic-model".into(),
            subject_id: SubjectId::parse("sub_00000001-0000-4000-8000-000000000001").unwrap(),
            max_candidates: 32,
            max_pending: 500,
            max_reserved_tokens: 100_000,
            max_reserved_cost_microusd: None,
        },
        b"review-history",
        &NeverCancel,
    )
    .unwrap()
}
fn golden() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/history/review-golden.json"
    ))
    .unwrap()
}
fn complete(
    env: &Env,
    api: Api,
    child: &enouia_memory_provider::history::HistoryBatchProgress,
    claims: &serde_json::Value,
    policy: PolicyId,
    apply: bool,
) -> RunSelection {
    let enabled = AtomicBool::new(true);
    let adapter = VaultAdapter {
        vault: &env.vault,
        owner: owner(),
        input_event: child.input_event_id.clone(),
        enabled: &enabled,
        quota: Quota::default(),
    };
    let (index, _) = enouia_memory_index::Index::rebuild(&env.vault).unwrap();
    let call = extraction::prepare(
        &adapter,
        &index,
        &child.run_id,
        &extraction_capabilities(env, api),
        Limits::default(),
        CallOptions::text(policy, 1024),
    )
    .unwrap();
    let mut wire: serde_json::Value = serde_json::from_slice(&reply(api)).unwrap();
    if api == Api::OpenAiResponses {
        wire["output"][0]["content"][0]["text"] = json!(claims.to_string());
    } else {
        wire["content"][0]["text"] = json!(claims.to_string());
    }
    let http = ReviewHttp(serde_json::to_vec(&wire).unwrap());
    let client = Client {
        transport: &http,
        secrets: &Secrets { missing: false },
        guard: &adapter,
        journal: &adapter,
    };
    client
        .send(&call, &call.inspect().hash(), &NeverCancel)
        .unwrap();
    if apply {
        for _ in claims["candidates"].as_array().unwrap() {
            extraction::apply_next(&adapter, &child.run_id).unwrap();
        }
    }
    RunSelection {
        session_id: child.session_id.clone(),
        run_id: child.run_id.clone(),
    }
}
fn accept(env: &Env, candidate: &CandidateRecord) {
    let shown = enouia_memory_govern::plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: candidate.candidate_id.clone(),
            revision: candidate.revision,
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    enouia_memory_govern::confirm(
        &env.vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
}

#[test]
fn golden_review_keeps_time_branches_negation_and_unaccepted_model_evidence() {
    for api in [Api::OpenAiResponses, Api::AnthropicMessages] {
        let env = Env::new("history-review-golden");
        let plan = batches(&env, api, vec![0, 1, 2, 3, 4, 5]);
        let policy = grant(&env, api);
        let fixture = golden();
        let selected = plan
            .batches
            .iter()
            .zip(fixture["runs"].as_array().unwrap())
            .map(|(child, run)| {
                complete(
                    &env,
                    api,
                    child,
                    &json!({"candidates":run["candidates"]}),
                    policy.clone(),
                    true,
                )
            })
            .collect::<Vec<_>>();
        let enabled = AtomicBool::new(true);
        let before = env.vault.pin_current().unwrap();
        let review = history_review::preview(&env.vault, &owner(), &selected, &enabled).unwrap();
        assert_eq!(review.base, before);
        assert_eq!(env.vault.pin_current().unwrap(), before);
        assert_eq!(review.topics.len(), 3);
        let pref = review
            .topics
            .iter()
            .find(|t| t.key.label == "explanation")
            .unwrap();
        assert_eq!(
            pref.entries.len(),
            fixture["expected"]["explanation_count"].as_u64().unwrap() as usize
        );
        assert_eq!(pref.comparisons[0].difference, Difference::DatedDifference);
        assert_eq!(pref.comparisons[0].branches, BranchRelation::SamePath);
        assert_eq!(
            pref.entries[0]
                .item
                .citation
                .occurred_at
                .as_ref()
                .unwrap()
                .as_str(),
            "2023-01-01T00:00:00.000Z"
        );
        let map = review.topics.iter().find(|t| t.key.label == "map").unwrap();
        assert_eq!(
            map.entries.len(),
            fixture["expected"]["map_count"].as_u64().unwrap() as usize
        );
        assert_eq!(
            map.comparisons.len(),
            fixture["expected"]["map_pairs"].as_u64().unwrap() as usize
        );
        assert_eq!(
            map.comparisons
                .iter()
                .filter(|p| p.difference == Difference::ExactRepeat)
                .count(),
            fixture["expected"]["repeat_pairs"].as_u64().unwrap() as usize
        );
        let alternate = map
            .entries
            .iter()
            .position(|e| e.item.proposal.content == "编辑分支保留地图。")
            .unwrap();
        let cancelled = map
            .entries
            .iter()
            .position(|e| e.item.proposal.content == "主分支取消地图且未采纳导航。")
            .unwrap();
        let pair = map
            .comparisons
            .iter()
            .find(|p| p.left == alternate && p.right == cancelled)
            .unwrap();
        assert_eq!(pair.branches, BranchRelation::Divergent);
        assert_eq!(pair.difference, Difference::DatedDifference);
        assert!(
            map.entries[cancelled]
                .item
                .citation
                .selected_text
                .contains("不采纳自动导航建议")
        );
        assert!(
            map.entries
                .last()
                .unwrap()
                .flags
                .contains(&ReviewFlag::UnknownTime)
        );
        assert!(
            map.comparisons
                .iter()
                .filter(|p| p.right == map.entries.len() - 1)
                .all(|p| p.difference == Difference::UnorderedDifference
                    && p.branches == BranchRelation::SeparateContexts)
        );
        assert!(
            map.comparisons
                .iter()
                .filter(|p| p.difference == Difference::ExactRepeat)
                .all(|p| p.branches == BranchRelation::SamePath)
        );
        let nav = review
            .topics
            .iter()
            .find(|t| t.key.label == "navigation")
            .unwrap();
        assert_eq!(
            nav.entries.len(),
            fixture["expected"]["navigation_count"].as_u64().unwrap() as usize
        );
        assert!(nav.entries[0].flags.contains(&ReviewFlag::NonUserEvidence));
        assert!(
            nav.entries[0]
                .item
                .citation
                .selected_text
                .contains("尚未被接受")
        );
        assert!(
            enouia_memory_govern::canonical_memories(&env.vault, &before, Canonical::Active)
                .unwrap()
                .is_empty()
        );
        assert!(history_review::preview(&env.vault, &agent(), &selected, &enabled).is_err());
        assert!(
            history_review::preview(
                &env.vault,
                &owner(),
                &[selected[0].clone(), selected[0].clone()],
                &enabled
            )
            .is_err()
        );
        enabled.store(false, Ordering::SeqCst);
        assert_eq!(
            history_review::preview(&env.vault, &owner(), &selected, &enabled)
                .unwrap_err()
                .code,
            MemoryErrorCode::VaultLocked
        );
        assert_eq!(env.vault.pin_current().unwrap(), before);
    }
}

#[test]
fn review_keeps_owner_edits_and_withdrawals_separate_from_original_response() {
    let env = Env::new("history-review-owner-edit");
    let api = Api::OpenAiResponses;
    let plan = batches(&env, api, vec![0]);
    let policy = grant(&env, api);
    let selected = complete(
        &env,
        api,
        &plan.batches[0],
        &json!({"candidates":golden()["runs"][0]["candidates"]}),
        policy,
        true,
    );
    let enabled = AtomicBool::new(true);
    let initial = history_review::preview(
        &env.vault,
        &owner(),
        std::slice::from_ref(&selected),
        &enabled,
    )
    .unwrap();
    let old = initial.topics[0].entries[0]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    let new = initial.topics[0].entries[1]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    enouia_memory_govern::edit_candidate(
        &env.vault,
        &new.candidate_id,
        new.revision,
        &CandidateEdit {
            content: Some("主人修订：先给结论，细节按需。".into()),
            ..CandidateEdit::default()
        },
        &owner(),
        b"edit-preference",
    )
    .unwrap();
    enouia_memory_govern::withdraw(
        &env.vault,
        &old.candidate_id,
        old.revision,
        &owner(),
        b"withdraw-old",
    )
    .unwrap();
    let before = env.vault.pin_current().unwrap();
    let review = history_review::preview(&env.vault, &owner(), &[selected], &enabled).unwrap();
    assert!(
        review.topics[0].entries[0]
            .flags
            .contains(&ReviewFlag::ReviewedOrWithdrawn)
    );
    assert_eq!(
        review.topics[0].entries[0]
            .item
            .current_candidate
            .as_ref()
            .unwrap()
            .status,
        CandidateStatus::Withdrawn
    );
    let row = &review.topics[0].entries[1];
    assert!(row.flags.contains(&ReviewFlag::OwnerEdited));
    assert_eq!(row.item.proposal.content, "现在希望先给简短结论。");
    assert_eq!(
        row.item
            .current_candidate
            .as_ref()
            .unwrap()
            .proposed_content,
        "主人修订：先给结论，细节按需。"
    );
    assert_eq!(env.vault.pin_current().unwrap(), before);
}

#[test]
fn explicit_supersession_requires_current_revisions_then_normal_owner_confirmation() {
    for effective in [
        BusinessTime::Unknown,
        BusinessTime::Known(Timestamp::from_unix_ms(1_740_000_000_000).unwrap()),
    ] {
        let env = Env::new("history-review-supersession");
        let api = Api::OpenAiResponses;
        let plan = batches(&env, api, vec![0]);
        let policy = grant(&env, api);
        let selected = complete(
            &env,
            api,
            &plan.batches[0],
            &json!({"candidates":golden()["runs"][0]["candidates"]}),
            policy,
            true,
        );
        let enabled = AtomicBool::new(true);
        let review = history_review::preview(&env.vault, &owner(), &[selected], &enabled).unwrap();
        let old = review.topics[0].entries[0]
            .item
            .current_candidate
            .as_ref()
            .unwrap();
        let new = review.topics[0].entries[1]
            .item
            .current_candidate
            .as_ref()
            .unwrap();
        accept(&env, old);
        let target = enouia_memory_govern::canonical_memories(
            &env.vault,
            &env.vault.pin_current().unwrap(),
            Canonical::Active,
        )
        .unwrap()
        .remove(0);
        enouia_memory_govern::edit_candidate(
            &env.vault,
            &new.candidate_id,
            new.revision,
            &CandidateEdit {
                content: Some("主人修订：先给结论，细节按需。".into()),
                ..CandidateEdit::default()
            },
            &owner(),
            b"edit-before-draft",
        )
        .unwrap();
        let changed =
            enouia_memory_govern::pending_candidates(&env.vault, &env.vault.pin_current().unwrap())
                .unwrap()
                .into_iter()
                .find(|c| c.candidate_id == new.candidate_id)
                .unwrap();
        let before = env.vault.pin_current().unwrap();
        assert!(
            history_review::supersession_draft(
                &env.vault,
                &owner(),
                &new.candidate_id,
                new.revision,
                &target.memory_id,
                target.revision,
                effective.clone(),
                &enabled
            )
            .is_err()
        );
        let draft = history_review::supersession_draft(
            &env.vault,
            &owner(),
            &new.candidate_id,
            changed.revision,
            &target.memory_id,
            target.revision,
            effective.clone(),
            &enabled,
        )
        .unwrap();
        assert_eq!(draft.base, before);
        assert_eq!(draft.proposal.content, "主人修订：先给结论，细节按需。");
        assert_eq!(draft.proposal.effective_from, Some(effective.clone()));
        assert_eq!(draft.origin.confidence, new.confidence);
        assert_eq!(draft.origin.actor, new.origin_actor);
        assert_eq!(
            draft.proposal.evidence[0].source_id,
            new.evidence[0].source_id
        );
        assert_eq!(env.vault.pin_current().unwrap(), before);
        let Proposed::Stored(written) = enouia_memory_govern::propose_at_head(
            &env.vault,
            &draft.proposal,
            &draft.origin,
            b"owner-selected-supersession",
            &draft.base.commit_id,
        )
        .unwrap() else {
            panic!("stored")
        };
        assert_eq!(
            enouia_memory_govern::canonical_memories(
                &env.vault,
                &env.vault.pin_current().unwrap(),
                Canonical::Active
            )
            .unwrap()[0]
                .memory_id,
            target.memory_id
        );
        let pending =
            enouia_memory_govern::pending_candidates(&env.vault, &env.vault.pin_current().unwrap())
                .unwrap()
                .into_iter()
                .find(|c| c.candidate_id == written.id)
                .unwrap();
        accept(&env, &pending);
        let pin = env.vault.pin_current().unwrap();
        let active = enouia_memory_govern::canonical_memories(&env.vault, &pin, Canonical::Active)
            .unwrap()
            .remove(0);
        assert_eq!(active.content, draft.proposal.content);
        assert_eq!(active.supersedes[0].effective_from, effective);
        let all =
            enouia_memory_govern::canonical_memories(&env.vault, &pin, Canonical::AllStatuses)
                .unwrap();
        assert!(
            all.iter()
                .any(|m| m.memory_id == target.memory_id && m.status == MemoryStatus::Superseded)
        );
        assert!(
            history_review::supersession_draft(
                &env.vault,
                &owner(),
                &new.candidate_id,
                changed.revision,
                &target.memory_id,
                target.revision,
                BusinessTime::Unknown,
                &enabled
            )
            .is_err()
        );
        assert!(env.vault.verify(&pin).unwrap().is_clean());
    }
}

#[test]
fn supersession_refuses_different_topic_and_stale_draft_publication() {
    let mut env = Env::new("history-review-stale-draft");
    let api = Api::OpenAiResponses;
    let plan = batches(&env, api, vec![0]);
    let policy = grant(&env, api);
    let selected = complete(
        &env,
        api,
        &plan.batches[0],
        &json!({"candidates":golden()["runs"][0]["candidates"]}),
        policy,
        true,
    );
    let enabled = AtomicBool::new(true);
    let review = history_review::preview(&env.vault, &owner(), &[selected], &enabled).unwrap();
    let old = review.topics[0].entries[0]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    let new = review.topics[0].entries[1]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    accept(&env, old);
    let target = enouia_memory_govern::canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .remove(0);
    let other_id = env.remember("unrelated", "other", "无关事实。", None);
    let other = enouia_memory_govern::canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .into_iter()
    .find(|m| m.memory_id == other_id)
    .unwrap();
    let before = env.vault.pin_current().unwrap();
    assert!(
        history_review::supersession_draft(
            &env.vault,
            &owner(),
            &new.candidate_id,
            new.revision,
            &other.memory_id,
            other.revision,
            BusinessTime::Unknown,
            &enabled
        )
        .is_err()
    );
    assert!(
        history_review::supersession_draft(
            &env.vault,
            &agent(),
            &new.candidate_id,
            new.revision,
            &target.memory_id,
            target.revision,
            BusinessTime::Unknown,
            &enabled
        )
        .is_err()
    );
    assert_eq!(env.vault.pin_current().unwrap(), before);
    let draft = history_review::supersession_draft(
        &env.vault,
        &owner(),
        &new.candidate_id,
        new.revision,
        &target.memory_id,
        target.revision,
        BusinessTime::Unknown,
        &enabled,
    )
    .unwrap();
    enouia_memory_govern::withdraw(
        &env.vault,
        &new.candidate_id,
        new.revision,
        &owner(),
        b"withdraw-after-preview",
    )
    .unwrap();
    let after = env.vault.pin_current().unwrap();
    assert!(
        enouia_memory_govern::propose_at_head(
            &env.vault,
            &draft.proposal,
            &draft.origin,
            b"stale-draft",
            &draft.base.commit_id
        )
        .is_err()
    );
    assert!(
        history_review::supersession_draft(
            &env.vault,
            &owner(),
            &new.candidate_id,
            new.revision,
            &target.memory_id,
            target.revision,
            BusinessTime::Unknown,
            &enabled
        )
        .is_err()
    );
    assert_eq!(env.vault.pin_current().unwrap(), after);
}

#[test]
fn review_refuses_missing_responses_and_comparison_overflow_without_writes() {
    let env = Env::new("history-review-bounds");
    let api = Api::OpenAiResponses;
    let plan = batches(&env, api, vec![0, 1, 2, 3]);
    let enabled = AtomicBool::new(true);
    let missing = RunSelection {
        session_id: plan.batches[0].session_id.clone(),
        run_id: plan.batches[0].run_id.clone(),
    };
    let before = env.vault.pin_current().unwrap();
    assert!(history_review::preview(&env.vault, &owner(), &[missing], &enabled).is_err());
    assert_eq!(env.vault.pin_current().unwrap(), before);
    let policy = grant(&env, api);
    let mut selected = vec![];
    for (i, child) in plan.batches.iter().enumerate() {
        let quote = if i == 0 { "合成角色" } else { "地图" };
        let claims = json!({"candidates":(0..32).map(|n| json!({"kind":"fact","content":format!("受控重复组 {i}-{n}"),"quote":quote,"source_index":0,"claim_key":"bounded"})).collect::<Vec<_>>()});
        selected.push(complete(&env, api, child, &claims, policy.clone(), false));
    }
    let before = env.vault.pin_current().unwrap();
    assert_eq!(
        history_review::preview(&env.vault, &owner(), &selected, &enabled)
            .unwrap_err()
            .code,
        MemoryErrorCode::BudgetExceeded
    );
    assert_eq!(env.vault.pin_current().unwrap(), before);
}

#[test]
fn supersession_refuses_changed_scope_subject_or_unresolved_project() {
    let env = Env::new("history-review-topic-boundary");
    let api = Api::OpenAiResponses;
    let plan = batches(&env, api, vec![0]);
    let policy = grant(&env, api);
    let selected = complete(
        &env,
        api,
        &plan.batches[0],
        &json!({"candidates":golden()["runs"][0]["candidates"]}),
        policy,
        true,
    );
    let enabled = AtomicBool::new(true);
    let review = history_review::preview(&env.vault, &owner(), &[selected], &enabled).unwrap();
    let old = review.topics[0].entries[0]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    let new = review.topics[0].entries[1]
        .item
        .current_candidate
        .as_ref()
        .unwrap();
    accept(&env, old);
    let target = enouia_memory_govern::canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .remove(0);
    let mut revision = new.revision;
    for (i, (field, value)) in [
        ("scope", json!("other-scope")),
        (
            "subject_ids",
            json!(["sub_00000002-0000-4000-8000-000000000002"]),
        ),
        (
            "project_id",
            json!("proj_00000002-0000-4000-8000-000000000002"),
        ),
        (
            "new_project",
            json!({"display_name":"Synthetic new project","aliases":[]}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut details = new.proposed_details.clone().unwrap();
        details.insert(field.into(), value);
        enouia_memory_govern::edit_candidate(
            &env.vault,
            &new.candidate_id,
            revision,
            &CandidateEdit {
                details: Some(details),
                ..CandidateEdit::default()
            },
            &owner(),
            format!("edit-topic-{i}").as_bytes(),
        )
        .unwrap();
        revision =
            enouia_memory_govern::pending_candidates(&env.vault, &env.vault.pin_current().unwrap())
                .unwrap()
                .into_iter()
                .find(|c| c.candidate_id == new.candidate_id)
                .unwrap()
                .revision;
        let before = env.vault.pin_current().unwrap();
        assert!(
            history_review::supersession_draft(
                &env.vault,
                &owner(),
                &new.candidate_id,
                revision,
                &target.memory_id,
                target.revision,
                BusinessTime::Unknown,
                &enabled
            )
            .is_err(),
            "{field}"
        );
        assert_eq!(env.vault.pin_current().unwrap(), before);
    }
}

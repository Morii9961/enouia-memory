//! Historical initialization preparation: synthetic bytes, no live accounts.
use enouia_memory_contract::common::{EvidenceClass, SpeakerRole};
use enouia_memory_contract::hash::sha256;
use enouia_memory_import::detect::{Detection, detect};
use enouia_memory_import::history::{
    HistoryIssueKind, HistoryLimits, HistoryPlanError, MessageRef, plan_history,
};
use enouia_memory_import::locate::resolve;
use enouia_memory_import::parsed::{ParentLink, ParsedInput};
use enouia_memory_import::zip::ZipLimits;
use std::collections::BTreeSet;

const BYTES: &[u8] =
    include_bytes!("../../../tests/fixtures/history/initialization-synthetic.json");

fn input() -> ParsedInput {
    let Detection::Parsed(input) = detect(BYTES, &ZipLimits::default()) else {
        panic!("synthetic fixture");
    };
    input
}

fn limits() -> HistoryLimits {
    HistoryLimits {
        messages_per_batch: 2,
        ..HistoryLimits::default()
    }
}

#[test]
fn received_coverage_is_exact_and_does_not_invent_unknown_time() {
    let input = input();
    let plan = plan_history(&input, limits()).unwrap();
    assert_eq!((plan.chronological.len(), plan.undated.len()), (6, 1));
    assert_eq!(plan.unparseable, 1);
    assert_eq!(plan.incomplete_messages, 1);
    assert_eq!(plan.unavailable_attachments, 1);
    assert_eq!(plan.earliest.unwrap().as_str(), "2023-01-01T00:00:00.000Z");
    assert_eq!(plan.latest.unwrap().as_str(), "2026-01-01T00:00:00.000Z");
    let all: BTreeSet<_> = plan.chronological.iter().chain(&plan.undated).collect();
    assert_eq!(all.len(), 7);
    assert_eq!(plan.chronological[0].unit, 1, "not export array order");
    assert!(
        input.units[plan.undated[0].unit].messages[plan.undated[0].message]
            .occurred_at
            .is_none()
    );
}

#[test]
fn branch_batches_keep_ancestors_corrections_and_export_selection_separate() {
    let input = input();
    let plan = plan_history(&input, limits()).unwrap();
    let project: Vec<_> = plan.batches.iter().filter(|b| b.leaf.unit == 0).collect();
    assert_eq!(project.len(), 4);
    let mut leaves = BTreeSet::new();
    for batch in &project {
        let leaf_id = input.units[0].messages[batch.leaf.message]
            .upstream_id
            .as_deref()
            .unwrap();
        leaves.insert(leaf_id);
        assert_eq!(batch.selected_in_export, leaf_id == "u2");
        let ids: Vec<_> = batch
            .messages
            .iter()
            .map(|r| {
                input.units[0].messages[r.message]
                    .upstream_id
                    .as_deref()
                    .unwrap()
            })
            .collect();
        assert!(!(ids.contains(&"edit") && ids.contains(&"u2")));
        if ids.contains(&"u2") || ids.contains(&"edit") {
            let previous = batch.preceding.unwrap();
            assert_eq!(
                input.units[0].messages[previous.message]
                    .upstream_id
                    .as_deref(),
                Some("a1")
            );
        }
    }
    assert_eq!(leaves, BTreeSet::from(["edit", "u2"]));
    assert_eq!(plan.batches[0].leaf.unit, 1, "oldest path scheduled first");
    assert_eq!(
        plan.batches.last().unwrap().leaf.unit,
        2,
        "undated path stays undated"
    );
}

#[test]
fn citations_resolve_exactly_and_model_suggestions_stay_model_claims() {
    let input = input();
    let plan = plan_history(&input, limits()).unwrap();
    for reference in plan.chronological.iter().chain(&plan.undated) {
        let message = &input.units[reference.unit].messages[reference.message];
        assert_eq!(
            sha256(&resolve(BYTES, &message.locator).unwrap()),
            message.content_hash
        );
        if message.upstream_id.as_deref() == Some("a1") {
            assert_eq!(message.role, SpeakerRole::Assistant);
            assert_eq!(message.evidence, EvidenceClass::ModelClaim);
        }
    }
    // The injection is included as source data, with no approval machinery.
    let unknown = &input.units[2].messages[0];
    assert!(
        String::from_utf8(resolve(BYTES, &unknown.locator).unwrap())
            .unwrap()
            .contains("自动批准")
    );
    assert_eq!(plan_history(&input, limits()).unwrap(), plan);
}

#[test]
fn selection_is_never_guessed_from_time_or_last_node() {
    for selected in [None, Some("a1".to_owned()), Some("absent".to_owned())] {
        let mut input = input();
        input.units[0].current_leaf = selected;
        let plan = plan_history(&input, limits()).unwrap();
        assert!(
            plan.issues
                .iter()
                .any(|i| i.unit == 0 && i.kind == HistoryIssueKind::UnknownSelection)
        );
        assert!(
            plan.batches
                .iter()
                .filter(|b| b.leaf.unit == 0)
                .all(|b| !b.selected_in_export)
        );
    }
}

#[test]
fn unsafe_graphs_remain_in_coverage_but_receive_no_batches() {
    for kind in 0..4 {
        let mut input = input();
        let unit = &mut input.units[0];
        match kind {
            0 => unit.messages[0].parent = ParentLink::Missing,
            1 => unit.messages[0].parent = ParentLink::Upstream("absent".into()),
            2 => {
                let id = unit.messages[0].upstream_id.clone().unwrap();
                unit.messages[0].parent = ParentLink::Upstream(id);
            }
            _ => unit.messages[1].upstream_id = unit.messages[0].upstream_id.clone(),
        }
        let plan = plan_history(&input, limits()).unwrap();
        assert_eq!(plan.chronological.len() + plan.undated.len(), 7);
        assert!(plan.batches.iter().all(|b| b.leaf.unit != 0));
        assert!(plan.issues.iter().any(|i| i.unit == 0));
    }
}

#[test]
fn disconnected_cycles_cannot_silently_disappear() {
    let mut input = input();
    let mut cycle = input.units[0].messages[0].clone();
    cycle.upstream_id = Some("cycle".into());
    cycle.parent = ParentLink::Upstream("cycle".into());
    input.units[0].messages.push(cycle);
    let plan = plan_history(&input, limits()).unwrap();
    assert_eq!(plan.chronological.len(), 7);
    assert!(plan.batches.iter().all(|b| b.leaf.unit != 0));
    assert!(
        plan.issues
            .iter()
            .any(|i| i.unit == 0 && i.kind == HistoryIssueKind::BrokenParentChain)
    );
}

#[test]
fn reply_order_survives_regressing_clocks() {
    let mut input = input();
    input.units[1].messages[1].occurred_at = input.units[0].messages[0].occurred_at.clone();
    input.units[1].messages[0].occurred_at = input.units[0].messages[3].occurred_at.clone();
    let plan = plan_history(&input, limits()).unwrap();
    assert!(
        plan.issues
            .iter()
            .any(|i| i.unit == 1 && i.kind == HistoryIssueKind::NonMonotonicTime)
    );
    let batch = plan.batches.iter().find(|b| b.leaf.unit == 1).unwrap();
    assert_eq!(
        batch.messages,
        vec![
            MessageRef {
                unit: 1,
                message: 0
            },
            MessageRef {
                unit: 1,
                message: 1
            }
        ]
    );
}

#[test]
fn reference_budget_counts_shared_ancestors_and_all_units() {
    let input = input();
    let plan = plan_history(&input, limits()).unwrap();
    let count: usize = plan.batches.iter().map(|b| b.messages.len()).sum();
    assert_eq!(
        count, 9,
        "shared sources are references, not duplicate imports"
    );
    assert_eq!(
        plan_history(
            &input,
            HistoryLimits {
                max_path_references: count - 1,
                ..limits()
            }
        ),
        Err(HistoryPlanError::ReferenceBudgetExceeded)
    );
    assert!(
        plan_history(
            &input,
            HistoryLimits {
                max_path_references: count,
                ..limits()
            }
        )
        .is_ok()
    );
}

#[test]
fn empty_unthreaded_and_invalid_limits_are_explicit() {
    let mut input = input();
    input.units.clear();
    let plan = plan_history(&input, limits()).unwrap();
    assert!(plan.earliest.is_none() && plan.latest.is_none() && plan.batches.is_empty());
    assert_eq!(
        plan_history(
            &input,
            HistoryLimits {
                messages_per_batch: 0,
                ..limits()
            }
        ),
        Err(HistoryPlanError::InvalidLimits)
    );
    assert_eq!(
        plan_history(
            &input,
            HistoryLimits {
                messages_per_batch: 257,
                ..limits()
            }
        ),
        Err(HistoryPlanError::InvalidLimits)
    );
    assert_eq!(
        plan_history(
            &input,
            HistoryLimits {
                max_path_references: 0,
                ..limits()
            }
        ),
        Err(HistoryPlanError::InvalidLimits)
    );
    let mut parsed = super_input();
    parsed.units[1].threaded = false;
    let plan = plan_history(&parsed, limits()).unwrap();
    assert!(
        plan.batches
            .iter()
            .filter(|b| b.leaf.unit == 1)
            .all(|b| !b.selected_in_export)
    );
}

fn super_input() -> ParsedInput {
    input()
}
